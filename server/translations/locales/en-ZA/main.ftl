ui-language = English (South Africa)
    .flag = 🇿🇦

nav = Navigation
    .site-icon = Site icon
    .menu = Menu
    .google-logo = Google logo
    .sign-in-with-google = Sign in with Google
    .log-out = Log out
    .user = User settings for { $username }

settings = Settings
    .save = Save
    .confirm = Are you sure you want to save these settings?
    .success = Successfully saved settings.
    .failure = There was an error saving settings.
    .unsaved = You have unsaved changes.

about = About
    .description = { site.short-name } is a free, open, online dictionary for { target-language } and { source-language }.
    .aim = Aim
    .aim-text = { site.aim-text }
    .usage = Usage
    .usage-text =
        To search for a word, start typing either the { source-language } or the { target-language } word in the search bar to show
        live search results. You can click on a word to find more information about the word, such as
        examples and related words. The dictionary currently has <strong>{ $word-count } entries.</strong>
        You can also download the words as an <a href="{ site.anki-url }">Anki flashcard deck</a>.
    .submitting-edits = Submitting edits
    .submitting-edits-text =
        <p>If you'd like to help the project, submitting new words and editing old ones would be greatly appreciated!
            First, you will need to create an account with the site. To do so, click the <a href="{ -oidc-link }">
            Sign in with Google</a> button in the top right. From your Google account, { site.short-name } will only
            record your email, in order for the team to email you about any issues regarding your account or
            the site, and your Google OpenID Connect ID, to identify you when you log in.
        </p>
    .submitting-edits-text =
        {"<p>"}If you'd like to help the project, submitting new words and editing old ones would be greatly appreciated!
            First, you will need to create an account with the site. To do so, click the <a href="{ -oidc-link }">
            Sign in with Google</a> button in the top right. From your Google account, { site.short-name } will only
            record your email, in order for the team to email you about any issues regarding your account or
            the site, and your Google OpenID Connect ID, to identify you when you log in.
        </p>

        <p>Once you have done this, to submit a new word, go to the <a href="/submit">Submit a word</a> tab and
            then enter all the information about the word. To edit a word, find the word, go to its details page,
            and click on the edit button. These suggestions will then be reviewed by moderators and eventually
            accepted or rejected.
        </p>

        <p>Please note that all contributions to the dictionary must be either your own, or reproduced with permission
            of the copyright holder under the { site.license-short } license.</p>
    .terms-of-use = Terms of use and privacy policy
    .terms-of-use-text =
        The terms of use and privacy policy of { site.short-name } are currently available <a href="/terms_of_use">here.</a>
    .license = License
    .license-text =
        <em>This section is a simple explanation of the license terms of { site.short-name } and in no way should be
        considered as legal advice. Please check the licenses linked below for the full terms and conditions.</em>

         <p>The database of { site.short-name} is licensed under the
            <a href="{ site.license-url} ">{ site.license-full }</a>. This ensures that the information of
            the dictionary is freely usable by all, as long as appropriate credit is given, and edits are also
            released under this license. Similarly, the software is licensed under the
            <a href="https://www.gnu.org/licenses/agpl-3.0.en.html">GNU Affero General Public License Version 3</a>.
        </p>

        <p>In simple terms, it means that you are free to</p>
            <ul>
                <li>copy</li>
                <li>reuse</li>
                <li>redistribute</li>
                <li>edit</li>
                <li>and build upon</li>
            </ul>
            the information and software of the dictionary, so long as you
            <ul>
                <li>give credit (for instance by providing a link)</li>
                <li>indicate if changes have been made</li>
                <li>distribute the edited version under the { site.license-short } license</li>
            </ul>

        <p>The source code is available from its <a href="https://github.com/IsiXhosa-click/isixhosa_click/">GitHub
        repository</a>, and the database of words, examples, and linked words is
        <a href="https://github.com/IsiXhosa-click/database/">available on GitHub.</a>
        The logo, designed by Jaydon Walters, is available under CC-BY-SA 4.0 in original resolution
        <a href="/icons/original.png">here.</a>
        </p>
    .acknowledgements = Acknowledgements
    .acknowledgements-text =
        <p>Thank you to the following people, without whom this project would not have been possible:</p>

        <ul>
            <li>
                <a href="https://ched.uct.ac.za/dot4d/implementation-dot4d-grantee-author-profiles/tim-low-statistics">
                <strong>Tim Low,</strong></a>
                for graciously allowing us to reproduce his isiXhosa statistics glossary on the site. Almost all
                entries relating to statistics are from there. We are currently working on a better way to attribute
                these words individually, but for now they should say "stats glossary" in the notes field.</li>
            <li><strong>Dr Tessa Dowling,</strong> for her guidance and many corrections.</li>
            <li><strong>Alexandra Bryant,</strong> for allowing us to use the glossary at the back of her
                excellent book "Xhosa for Second-Language Learners".</li>
            <li><strong>Cuan Dugmore,</strong> for giving us permission to use his word lists containing
                thousands of words that he has collected over the years.</li>
            <li><a href="https://sit.uct.ac.za/department-computer-science">The UCT Department of Computer Science</a>,
                for hosting the site's server.</li>
            <li><a href="https://sit.uct.ac.za/contacts/craig-balfour"><strong>Craig Balfour,</strong></a> for his
                assistance in setting up the server at UCT.</li>
        </ul>
    .contact = Contact
    .contact-text =
        <p>You can get in touch with us on
            <a href="https://join.slack.com/t/isixhosaclick/shared_invite/zt-17xo8auw6-f05kut4xJBCaEFhM5I7nqw">
                our Slack workspace.</a> Anyone is free to join and there are no obligations or requirements
        except to act respectfully 🙂.</p>
        <address>Otherwise, feel free to get in touch by email:
            <a href="mailto:restiosondev@gmail.com">restiosondev@gmail.com</a>.
        </address>

search = Search
    .do-search = Search
    .header = Search for a word
    .description = Search for a word in the free, open { site.short-name } dictionary for { target-language } and { source-language }.
    .prompt = Type {{ source-language.indef-article }} or {{ target-language }} word
    .no-results = No results.

submit = Submit a word
    .description = Submit a word to the free, open, online { site.short-name } dictionary for { target-language } and { source-language }.
    .submit-success = Word successfully submitted!
    .submit-fail = There was an error submitting the word.
    .check-style = Take a look at the <a href="{ -style-guide-url }">style guide</a> before submitting a word.
    .required-field = Required fields are marked with a <span class="required">*</span>.
    .translation = Translation
    .possible-duplicates = Possible duplicates (hover)
    .select-plural = Plural?
    .select-inchoative = Inchoative?
    .add-example = Add another
    .add-linked-word = Add another
    .submit-new = Suggest word
    .submit-edit-suggestion = Submit edit to suggestion
    .submit-edit = Suggest edit to word

changes = Changes made and why
    .explanation = Briefly explain the changes you made and why.

js-required = Javascript is required for this page to work properly due to the complex nature of the form. Apologies!
    .wasm = WebAssembly is required to play. Sorry
    .both = JavaScript and WebAssembly are required to play. Sorry!

wordle = Wordle
    .name = { target-language } Wordle (beta)
    .description = { target-language } Wordle on { site.short-name }!
    .share = Share with your friends
    .share-title = { target-language } Wordle v2 { $nth-wordle } { $score } / { $guesses }
    .victory = Congratulations! A new wordle will be available tomorrow.
    .loss = Unlucky... try again tomorrow.
    .copied = Copied!
    .word-was = Today's word was

moderation = Moderation
    .possible-duplicates = Possible duplicates
    .suggestions = Suggestions
    .suggested-words = Suggested words
    .deletion-suggestions = Deletion suggestions
    .reason = Reason
    .details = Details
    .removed = Removed
    .none = None
    .change-type = Change type
    .word-deleted = Word deleted
    .suggestor = Suggested by
    .suggested-by = <strong>Suggested by</strong> { $username }
    .changes-summary = Changes summary
    .selected-class = Selected noun class
    .edit = Edit
    .accept = Accept
    .reject = Reject
    .accept-deletion = Accept deletion
    .reject-deletion = Reject deletion
    .edited-examples-and-links = Edited examples and linked words
    .no-suggestions = There are no suggestions to review at this time.
    .action-success =
        {$method ->
            [accept] Successfully accepted suggestion.
            [edit] Successfully edit suggestion.
            [reject] Successfully rejected suggestion.
            *[other] Success.
        }
    .action-fail =
        An error occurred {$method ->
            [accept] while accepting a suggestion.
            [edit] while editing a suggestion.
            [reject] white rejecting a suggestion.
            *[other] .
        }
    .confirm-action = Are you sure you want to { $method } this suggestion?
    .confirm-delete =
        Are you sure you want to permanently delete this
        { $item ->
            [word] word
            [linked-word] linked word
            [example] example
           *[other] suggestion
        }?
    .confirm-reject = Are you sure you want to reject this suggestion?

tracing = Tracing

share = Share

word = Word details page
    .information = Word information
    .link-copied = Link copied!
    .suggest-edit = Suggest edit
    .suggest-delete = Suggest deletion
    .confirm-delete = Are you sure you want to suggest this word be deleted?
    .success-message =
        Successfully {$action ->
            [edit] suggested edit
           *[delete] suggested deletion
        }.

        It will be reviewed by moderators shortly, thank you!

transitivity = Transitivity
    .explanation = Whether the verb can take a direct object or not.

transitive = transitive
    .explicit = transitive-only
    .in-word-result = transitive
intransitive = intransitive
    .explicit = intransitive-only
    .in-word-result = intransitive
ambitransitive = either
    .explicit = ambitransitive
    .in-word-result ={""}

word-hit = Word hit, e.g as it appears in the search box
    .grammar-info =
        { $plural ->
            [true] plural
           *[other]{""}
        }
        { $informal ->
            [true] informal
           *[other]{""}
        }
        { $inchoative ->
            [true] inchoative
           *[other]{""}
        }
        { $transitivity ->
            [transitive] { transitive.in-word-result }
            [intransitive] { intransitive.in-word-result }
            [ambitransitive] { ambitransitive.in-word-result }
           *[other]{""}
        }
        { $part-of-speech ->
            [verb] { verb }
            [noun]
                    { noun }
                    { $class ->
                        [none] {""}
                       *[any] - class { $class }
                    }
            [adjective] { adjective }
            [adverb] { adverb }
            [relative] { relative }
            [interjection] { interjection }
            [conjunction] { conjunction }
            [preposition] { preposition }
            [ideophone] { ideophone }
            [boundmorpheme] { boundmorpheme }
           *[other]{""}
        }
    .class = class

followed-by = Followed by
    .explanation = The verbial mood or construction that this word is followed by.
    .indicative = indicative mood
    .subjunctive = subjunctive mood
    .participial = participial mood

with-tone-markings = { target-language } with tone markings
    .explanation = { target-language } with tone markings written out as diacritics. For example: "bónákala".

linked-words = Linked words
    .link-type = Link type
    .other-word = Other word
    .plurality = Singular or plural form
    .antonym = Antonym
    .related = Related meaning
    .confusable = Confusable
    .alternate = Alternate Use
    .choose = Choose how the words are related
    .search = Search for a linked word...

informal = Informal or slang?
    .in-word-result = informal
    .non = non-informal

yes = yes
    .capital = Yes
no = no
    .capital = No

note = Note

no-grammatical-info =
    This word doesn't have any further information yet. You can help by <a href="{ -edit-url-start }{ $word }{ -edit-url-end }">editing this entry</a>.

noun-class = Noun class
    .choose = Choose a noun class
    .in-word-result = class
infinitive = Infinitive
    .form = Infinitive form

inchoative = Inchoative
    .explanation =
        An inchoative (stative) verb takes the perfect tense for present tense meaning. For example, "ndilambile"
        means "I am hungry", whereas "ndiyalamba" means "I am getting hungry".
    .in-word-result = inchoative
    .non = non-inchoative

plurality = Plurality
    .plural = plural
    .singular = singular

part-of-speech = Part of speech
    .choose = Choose a part of speech

verb = verb
    .capitalised = Verb
noun = noun
    .capitalised = Noun
adjective = adjective (isiphawuili)
    .capitalised = Adjective (isiphawuili)
adverb = adverb
    .capitalised = Adverb
relative = relative (adjective)
    .capitalised = Relative (adjective)
interjection = interjection
    .capitalised = Interjection
conjunction = conjunction
    .capitalised = Conjunction
preposition = preposition
    .capitalised = Preposition
ideophone = ideophone
    .capitalised = Ideophone
boundmorpheme = bound morpheme
    .capitalised = Bound morpheme

examples = Example sentences
    .source = { source-language } example
    .target = { target-language } example

delete = Delete

contributors = Contributors

not-found = Page not found
    .sorry = This page was not found. Sorry!

offline = Offline
    .header = You are currently offline
    .explanation =
        You are currently disconnected from the internet, or the website could not be reached. Try reconnect to
        a network, or try again later.
    .click-to-reload = Click the button below to reload this page.

reload = Reload

redirect = Redirecting...
    .click-here = Click here
    .if-too-long = If this is taking too long
    .explanation = Redirecting you to the site.

sign-up = Sign up
    .error =
        An error occurred while signing up:
        {$reason ->
           *[invalid-username] invalid username. Usernames can only contain letters, numbers, spaces, underscores, and
                                hyphens, and they must be at least 2 characters and no more than 128 characters.
            [did-not-agree] you must agree to the Terms of Use and Privacy Policy to create an account on
                             { site.short-name }
            [no-email] the account you signed in to is not connected to any verified email addresses. Your email is
                       required in case { site.short-name } moderators or admins need to contact you about the status of
                       your account or for legal reasons. Try to log in again with a different Google account.
        }
    .terms-of-use-agreement =
        I agree to the <a href="{ -tou-url }">terms of use and privacy policy</a> of { site.short-name },
        and verify that I am over the age of 13.

select-language = Select <strong>interface</strong> language
    .explanation =
        This is the language that will be used for the user interface of the site. It does not change anything about the
        language of entries in the dictionary.

license-agreement =
     I agree that any content which I submit I will allow { site.short-name } to use subject to
     the <a href="{ -tou-url }">terms of use</a> and under the
     <a href="{ site.license-url }">{ site.license-full }</a>.

username = Display name
    .explanation = This is the name that others will see you by, and what you will be credited with for your submissions.
    .placeholder = John Doe
    .do-not-credit =
        Do not publicly credit me or my display name for my submissions, on the word page or anywhere else. Note that if
        you select this option, you waive your right to be attributed for your edits - see the Terms of Service for more
        information.

style-guide = Style guide
    .description = The style guide for entries on { site.short-name }.
    .entry-guidelines = Entry guidelines
    .entry-definition = An <dfn>entry</dfn> is a single page for a word.
    .entry-guidelines-text =
        <ul>
            <li>Each entry should describe one distinct meaning of a word</li>
            <li>If a word has two or more alternate meanings (e.g hamba - go and hamba - walk),
                <strong>two separate entries should be created.</strong></li>
            <li>These entries can then be used using the <a href="#linked-word-dfn">linked word feature</a>,
                using the <a href="#alternate-use-dfn">alternate use</a> link type.</li>
            <li>If the word is uncommon, informal, or archaic, this should be made clear in the note section.</li>
        </ul>
    .translation-preferences =
        Here are some style preferences about writing the { source-language } and { target-language } translations for
        an entry:
    .translation-preferences-text =
        <ul>
            <li>Do <strong>not</strong> capitalise the first letter of an entry, unless it is a proper noun
                (e.g Cape Town - iKapa).</li>
            <li>Extra information about a words usage can be put in the note field or shown by the examples.</li>
            <li>Do <strong>not</strong> prefix a verb with a dash. Prefer go - hamba to go - -hamba.
                This may be acceptable in cases where a verb is immediately followed by a bound morpheme like
                na-, for instance in the entry of <a href="https://isixhosa.click/word/89">brush against - thintana na-</a></li>
            <li>Try to come up with a close { source-language } translation for the { target-language } word. If there
                are multiple suitable options which are all in common usage, you might consider making multiple
                <a href="#entry-dfn">entries</a> and <a href="#linked-word-dfn">linking them</a> with the
                <a href="#related-meaning-dfn">related meaning</a> link type.</li>
            <li>For inchoative verbs representing a subject becoming some state,
                put the word become in the { source-language }. For instance,
                <a href="https://isixhosa.click/word/241">become injured - enzakala</a>.</li>
        </ul>
    .linked-words-definition =
         <dfn>Linked words</dfn> are a core concept of the site. They allow for words to be linked to one another.
         For instance, singular and plural words can be linked together, or synonyms.
    .linked-words-types = Types of linked words
    .linked-words-types-text =
        <ul>
            <li id="singular-or-plural-dfn"><dfn>Singular or plural form:</dfn> this is used to link two words which are
                related by plurality. E.g: <a href="https://isixhosa.click/word/504">ixesha</a> and
                <a href="https://isixhosa.click/word/505">amaxesha</a>.</li>
            <li id="alternate-use-dfn"><dfn>Alternate use:</dfn> this is used to link two words which have two or
                 more slightly different meanings in either language, however they are still the same word.
                 E.g: <a href="https://isixhosa.click/word/257">need - funa</a> and <a href="https://isixhosa.click/word/236">want - funa</a>.</li>
            <li id="related-meaning-dfn"><dfn>Related meaning:</dfn> this is used to link synonyms, words from the
                same root, or otherwise similarly-meaning words. E.g: <a href="https://isixhosa.click/word/64">funda</a> and
                <a href="https://isixhosa.click/word/221">fundisa</a>, <a href="https://isixhosa.click/word/86">thanda</a> and
                <a href="https://isixhosa.click/word/33">isithandwa</a>, and <a href="https://isixhosa.click/word/123">sika</a> and
                <a href="https://isixhosa.click/word/156">cheba</a>.</li>
            <li id="antonym-dfn"><dfn>Antonym:</dfn> this is used to link words with opposite meanings to eachother.
                E.g: <a href="https://isixhosa.click/word/149">khumbula</a> and <a href="https://isixhosa.click/word/130">libala</a>.</li>
            <li id="confusable-dfn"><dfn>Confusable:</dfn> unrelated words which could be confused based on how they
                sound or are spelled. E.g: <a href="https://isixhosa.click/word/39">ibali</a> and
                <a href="https://isixhosa.click/word/38">ibala</a>.</li>
        </ul>
    .examples = Example sentences
    .examples-text =
        <p>Example sentences are another core feature of the site. They help to show the use of the word in a real
        sentence, and can clarify meaning of the word. Try and abide by the following rules when writing examples:</p>

        <ul>
            <li>Submit full sentences rather than just phrases. E.g: "Intlanzi idada echibini." is a better
            example for <a href="https://isixhosa.click/word/406">intlanzi</a> than "Intlanzi enkulu".</li>
            <li>Use punctuation at the end of an examples, as well as proper spelling and grammar.</li>
            <li>Try and add at least one example per word.</li>
            <li>Examples should be used to fully demonstrate the meanings and usages of a word. E.g:
                "Ihashe libaleka ebaleni." is a better example for <a href="https://isixhosa.click/word/38">ibala</a> than
                "Ibala likhulu." as the first example shows it is a field of grass rather than a field of study.</li>
            <li>Therefore, avoid examples which are simplistic like "I do [verb]." or "I like [noun]."</li>
            <li>If a word has a common construction it's often used with, add it in an example!</li>
        </ul>

terms-of-use = Terms of use
    .description = The terms of use and privacy policy of { site.short-name }.
    .tos = Terms of service
    .last-update = The following terms of service and privacy policy were last updated on the 20th of May, 2024.
    .age =
        IsiXhosa.click's submission features are intended for use by people over the age of 13 only.
        By registering on the site <strong>you confirm that you are over the age of 13.</strong> To be clear,
        users under the age of 13 <strong>are allowed to use the site,</strong> but not register or submit words.
    .copyright =
        The <strong>copyright and ownership</strong> of all content submitted <strong>remains with the
        submitter or owner.</strong> The submitter grants the website <strong>irrevocable, indefinite permission
        to use</strong> all content submitted under the
        <strong><a href="{ site.license-url }">{ site.license-full }</a></strong>, or later versions thereof.
        You waive the right to attribution if you select that your name should not be displayed.
    .confirm-ownership =
        By submitting content, you <strong>confirm that you have permission</strong>
        to grant this license to the website under the specified terms.
    .privacy-policy = Privacy policy
    .info-collected = Information we collect
    .info-collected-text =
        We collect your <strong>email, username</strong>, and Google OpenID Connect id. This is so we can

        <ol>
            <li><strong>contact you</strong> in case of any issues regarding your account or submissions</li>
            <li><strong>credit you</strong> for your edits to the dictionary. This is stored on our server in
                the database. <strong>The username is also mirrored to the
                <a href="https://github.com/IsiXhosa-Click/database">public GitHub archive</a></strong>,
                in order to provide accreditation.
            </li>
        </ol>
    .email-disclosure =
        Your <strong>email is not disclosed</strong> to anyone except moderators and administrators of the site.
    .crediting =
        Your user id (generated by the website) and username (entered by you) is uploaded publicly to
        GitHub, in order to credit your submissions. If you choose to forgo crediting, then your name
        will not be displayed publicly on any page.
    .forgo-crediting =
        If you decide that you no longer wish to be credited, your name will be hidden and it will not
        be uploaded in future. However, it <strong>will remain in the history</strong> of the repository.
    .agree-crediting = If you decide that you do wish to be credited, your name will be shown on future and past edits.
    .logging =
        Your <strong>IP address and user agent are logged anonymously</strong> when you visit the site,
        in accordance with the industry standard. This is in order to gather data about how people use
        the site and diagnose any issues.
    .rights = Your rights
    .request-data = You may <strong>request all the data</strong> that the site has collected on you at any time.
    .correct-data = You may request that the data we hold on you is <strong>corrected</strong> at any time.
    .delete-data =
        You may request that your <strong>personal info is expunged</strong> by us at any time. However,
        any submissions that you have made will remain on the site. You will no longer be credited
        publicly on the site itself. However, the credit information and your username will remain in
        the history of the database online on GitHub.
    .contact-us =
         Please do so by <strong>contacting us</strong> according to the information listed on the
         <a href="/about">about page</a>, or send an email to
         <a href="mailto:restiosondev@gmail.com">restiosondev@gmail.com</a>.
